use super::process_tests::{FIXTURE_TIMEOUT, spawn_fake_chat};
use super::{ChatEvent, Event, EventQueue, Incoming, PiChat, SpawnConfig};

#[tokio::test]
async fn missing_pi_executable_reports_the_os_error() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing-pi");
    let expected = std::fs::metadata(&missing).unwrap_err();
    assert_eq!(expected.kind(), std::io::ErrorKind::NotFound);
    let (sender, _receiver) = tokio::sync::mpsc::channel(1);
    let result = PiChat::spawn(
        SpawnConfig {
            executable: &missing,
            workspace: directory.path(),
            bridge: directory.path(),
            sessions: directory.path(),
            saved: None,
            title: None,
        },
        1,
        sender,
    )
    .await;
    let Err(error) = result else {
        panic!("a missing executable must fail to start")
    };
    assert!(error.contains(&expected.to_string()), "{error}");
}

#[tokio::test]
async fn fake_rpc_process_receives_bounded_startup_queries_without_a_model_prompt() {
    let script = r#"#!/bin/sh
i=0
while [ "$i" -lt 3 ] && IFS= read -r line; do
  i=$((i + 1))
  case "$line" in
    *get_state*) printf '%s\n%s\n' '{"type":"response","id":"startup-state","command":"get_state","success":true,"data":{"sessionId":"fake-1","sessionFile":"/fake/session.jsonl","model":{"provider":"unknown","id":"unknown"},"thinkingLevel":"off"}}' '{"type":"response","id":"startup-state","command":"get_state","success":true,"data":{"sessionId":"fake-1","sessionFile":"/fake/session.jsonl","model":{"provider":"unknown","id":"unknown"},"thinkingLevel":"off"}}' ;;
    *get_available_models*) printf '%s\n' '{"type":"response","id":"startup-models","command":"get_available_models","success":true,"data":{"models":[]}}' ;;
    *get_messages*) printf '%s\n' '{"type":"response","id":"startup-history","command":"get_messages","success":true,"data":{"messages":[]}}' ;;
  esac
done
"#;
    let (_directory, mut chat, mut receiver) = spawn_fake_chat(script, 7).await;
    let events = EventQueue::default();
    for index in 0..4 {
        let (chat_id, Incoming::Record(record)) =
            tokio::time::timeout(FIXTURE_TIMEOUT, receiver.recv())
                .await
                .unwrap()
                .unwrap()
        else {
            panic!("expected startup response")
        };
        assert_eq!(chat_id, 7);
        assert_ne!(record["command"], "prompt");
        chat.receive(&record, &events).await.unwrap();
        let ready = super::super::lock(&events.items).iter().any(|event| {
            matches!(
                event,
                Event::Chat {
                    event: ChatEvent::Ready { .. },
                    ..
                }
            )
        });
        assert_eq!(
            ready,
            index == 3,
            "Ready must follow state, models and history"
        );
    }
    chat.stop().await;
}
