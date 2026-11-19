use super::tests::{settle, setup};
use crate::agent_chat::tabs::LastTurn;
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_agent::failure::TurnContext;
use subspace_agent::{ChatEvent, ContextRef, Event, FailureCategory, TurnFailure};
use subspace_core::log::{self, Level, Record};

fn checkpoint() -> u64 {
    log::recent().last().map_or(0, |record| record.seq)
}

fn records(seq: u64, message: &str) -> Vec<Record> {
    log::recent_after(seq)
        .into_iter()
        .filter(|record| {
            record.target == "agent"
                && record
                    .fields
                    .iter()
                    .any(|(key, value)| key == "message" && value.contains(message))
        })
        .collect()
}

fn turn_failure(failure: TurnFailure) -> Event {
    Event::Chat {
        chat: 1,
        event: ChatEvent::Failure(Box::new(failure)),
    }
}

#[gpui_kit::test]
fn accepted_turn_failure_logs_once_and_a_later_identical_failure_logs_again(
    cx: &mut TestAppContext,
) {
    let (chat, cx) = setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let (handle, _commands) =
        subspace_agent::AgentHandle::test_offline(directory.path().into(), Vec::new());
    let message = "host turn occurrence fixture";
    let failure = TurnFailure::stopped(
        FailureCategory::NoResponse,
        message,
        &TurnContext::default(),
        None,
    );
    let seq = checkpoint();
    chat.update(cx, |chat, cx| {
        chat.connected(Ok(handle));
        chat.receive(turn_failure(failure.clone()), cx);
    });
    settle(cx);
    cx.update(|window, cx| chat.update(cx, |chat, cx| chat.on_open(window, cx)));
    cx.update(|window, cx| window.click("chat-failure-copy", cx));
    let copied = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    chat.update(cx, |chat, cx| chat.retry(cx));
    settle(cx);
    let first = records(seq, message);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].level, Level::Error);
    assert!(
        first[0]
            .fields
            .contains(&("surface".into(), "chat-failure".into()))
    );
    assert!(first[0].fields.contains(&("details".into(), copied)));
    chat.update(cx, |chat, cx| {
        chat.tab_mut().begin();
        chat.receive(turn_failure(failure), cx);
        chat.receive(
            Event::Chat {
                chat: 999,
                event: ChatEvent::Error(message.into()),
            },
            cx,
        );
    });
    assert_eq!(
        records(seq, message).len(),
        2,
        "closed-tab failure is ignored"
    );
}

#[gpui_kit::test]
fn accepted_legacy_error_retains_observation_and_removes_private_turn_text(
    cx: &mut TestAppContext,
) {
    let (chat, cx) = setup(cx);
    let message = "host legacy occurrence fixture";
    let seq = checkpoint();
    chat.update(cx, |chat, cx| {
        chat.tab_mut().last_turn = Some(LastTurn {
            text: "PRIVATE TEST PROMPT".into(),
            context: vec![ContextRef {
                key: "test".into(),
                label: "test".into(),
                content: "PRIVATE TEST CONTEXT".into(),
            }],
            images: Vec::new(),
        });
        chat.receive(
            Event::Chat {
                chat: 1,
                event: ChatEvent::Error(format!(
                    "{message} PRIVATE TEST PROMPT PRIVATE TEST CONTEXT token=test-only-secret"
                )),
            },
            cx,
        );
    });
    settle(cx);
    cx.update(|window, cx| window.click("chat-error-copy", cx));
    let first = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    settle(cx);
    cx.update(|window, cx| window.click("chat-error-copy", cx));
    let second = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert_eq!(first, second);
    let logged = records(seq, message);
    assert_eq!(logged.len(), 1);
    assert!(
        logged[0]
            .fields
            .contains(&("surface".into(), "chat-error".into()))
    );
    assert!(logged[0].fields.contains(&("details".into(), first)));
    let serialized = logged[0].line();
    for private in [
        "PRIVATE TEST PROMPT",
        "PRIVATE TEST CONTEXT",
        "test-only-secret",
    ] {
        assert!(!serialized.contains(private), "{serialized}");
    }
    chat.update(cx, |chat, cx| {
        chat.receive(Event::Error(message.into()), cx)
    });
    assert_eq!(records(seq, message).len(), 2);
}
