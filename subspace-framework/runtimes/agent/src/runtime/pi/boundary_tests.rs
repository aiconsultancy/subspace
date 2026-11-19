//! Runtime adapter checks using the real batch executor and Pi subprocess boundary.
use super::{Catalog, Incoming, PiActor, bridge_request, test_actor};
use crate::runtime::{batch_fixture, lock};
use crate::{ChatEvent, Command, Event};
use serde_json::{Value, json};
use std::sync::Arc;

async fn approve(actor: &mut PiActor, request_id: String, allow: bool) {
    actor
        .apply(Command::Approve { request_id, allow }, &Default::default())
        .await
        .unwrap();
}

async fn tool_result(actor: &mut PiActor) -> (u64, Value, Value) {
    let (chat, Incoming::ToolResult { request, result }) =
        tokio::time::timeout(std::time::Duration::from_secs(10), actor.incoming.recv())
            .await
            .unwrap()
            .unwrap()
    else {
        panic!("bridge result");
    };
    (chat, request, result.unwrap())
}

#[tokio::test]
async fn bridge_routes_real_service_batch_approval_and_result_to_the_asking_chat() {
    let dir = tempfile::tempdir().unwrap();
    let tool = batch_fixture::tool();
    let service = tool.service().clone();
    let mut actor = test_actor(dir.path(), Catalog::new(vec![Arc::new(tool)]).unwrap());
    let mut request = bridge_request();
    request["id"] = json!("bridge-chat-nine");
    request["prefill"] = json!(batch_fixture::arguments().to_string());
    actor.call_tool(9, request.clone());
    let (chat, id, summary) = batch_fixture::approval(&actor.events).await;
    assert_eq!(chat, 9);
    assert!(summary.contains("first complete body") && summary.contains("second complete body"));
    assert!(summary.ends_with("Resolved 2 fixture notes"));
    assert!(service.written().is_empty());
    approve(&mut actor, "wrong-request".into(), true).await;
    assert!(actor.incoming.try_recv().is_err());
    approve(&mut actor, id, true).await;
    let (chat, returned, result) = tool_result(&mut actor).await;
    assert_eq!(chat, 9);
    assert_eq!(
        returned, request,
        "extension request identity survives approval"
    );
    assert_eq!(result["approved"], true);
    assert_eq!(
        service.written(),
        ["first complete body", "second complete body"]
    );
    assert!(
        lock(&actor.events.items).is_empty(),
        "the full batch asks once"
    );
}

#[tokio::test]
async fn denying_the_real_service_batch_through_pi_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let tool = batch_fixture::tool();
    let service = tool.service().clone();
    let mut actor = test_actor(dir.path(), Catalog::new(vec![Arc::new(tool)]).unwrap());
    let mut request = bridge_request();
    request["prefill"] = json!(batch_fixture::arguments().to_string());
    actor.call_tool(8, request);
    let (chat, id, _) = batch_fixture::approval(&actor.events).await;
    assert_eq!(chat, 8);
    assert!(service.written().is_empty());
    approve(&mut actor, id, false).await;
    let (chat, _, result) = tool_result(&mut actor).await;
    assert_eq!(chat, 8);
    assert_eq!(result["approved"], false);
    assert!(service.written().is_empty());
}

#[cfg(unix)]
async fn receive_next(actor: &mut PiActor) {
    let (id, response) = tokio::time::timeout(
        super::super::process_tests::FIXTURE_TIMEOUT,
        actor.incoming.recv(),
    )
    .await
    .unwrap()
    .unwrap();
    actor.receive(id, response).await;
}

/// The fake Pi's startup answers, then `prompts`: what it says to a prompt,
/// with the prompt's own request id in place of `%s`.
#[cfg(unix)]
fn fake_pi(prompts: &str) -> String {
    let startup = r#"    *startup-state*) printf '%s\n' '{"type":"response","id":"startup-state","command":"get_state","success":true,"data":{"sessionId":"provider-session","model":{"provider":"test-provider","id":"fixture"},"thinkingLevel":"off"}}' ;;
    *startup-models*) printf '%s\n' '{"type":"response","id":"startup-models","command":"get_available_models","success":true,"data":{"models":[{"provider":"test-provider","id":"fixture"}]}}' ;;
    *startup-history*) printf '%s\n' '{"type":"response","id":"startup-history","command":"get_messages","success":true,"data":{"messages":[]}}' ;;
"#;
    format!(
        "#!/bin/sh\nwhile IFS= read -r line; do\n  id=$(printf '%s' \"$line\" | sed -n 's/.*\"id\":\"\\(prompt-[0-9]*\\)\".*/\\1/p')\n  case \"$line\" in\n{startup}{prompts}  esac\ndone\n"
    )
}

#[cfg(unix)]
async fn started_fake_chat(prompts: &str) -> (tempfile::TempDir, PiActor) {
    let (directory, chat, incoming) =
        super::super::process_tests::spawn_fake_chat(&fake_pi(prompts), 6).await;
    let mut actor = test_actor(directory.path(), Catalog::new(Vec::new()).unwrap());
    actor.incoming = incoming;
    actor.chats.insert(6, chat);
    actor.order.push(6);
    for _ in 0..3 {
        receive_next(&mut actor).await;
    }
    assert!(actor.chats[&6].startup == crate::runtime::pi::Startup::Ready);
    (directory, actor)
}

#[cfg(unix)]
async fn send(actor: &mut PiActor, text: &str) {
    actor
        .chat_command(
            6,
            crate::ChatCommand::Send {
                text: text.into(),
                context: Vec::new(),
                images: Vec::new(),
            },
        )
        .await
        .unwrap();
}

#[cfg(unix)]
fn chat_events(actor: &PiActor) -> Vec<ChatEvent> {
    lock(&actor.events.items)
        .iter()
        .filter_map(|event| match event {
            Event::Chat { chat: 6, event } => Some(event.clone()),
            _ => None,
        })
        .collect()
}

#[cfg(unix)]
#[tokio::test]
async fn provider_authentication_rejection_reaches_the_asking_chat_without_github_credentials() {
    let (_directory, mut actor) = started_fake_chat(
        r#"    *prompt-*) printf '{"type":"response","id":"%s","command":"prompt","success":false,"error":"Provider authentication rejected: test-only expired credential"}\n' "$id" ;;
"#,
    )
    .await;
    send(&mut actor, "fixture question").await;
    assert!(actor.chats[&6].turn.is_running());
    receive_next(&mut actor).await;
    assert!(!actor.chats[&6].turn.is_running());
    let events = chat_events(&actor);
    let refused = events.iter().position(|event| {
        matches!(event, ChatEvent::Error(error) if error.contains("Provider authentication rejected"))
    });
    assert!(
        refused.is_some_and(|index| matches!(events.get(index + 1), Some(ChatEvent::TurnDone))),
        "the refused prompt ends the turn the panel started: {events:?}"
    );
    assert!(
        !lock(&actor.events.items)
            .iter()
            .any(|event| matches!(event, Event::SignInRequired(..)))
    );
    actor.shutdown().await;
}

/// Stop, then a steering message Pi refuses: the turn keeps running and
/// stays cancelled, and only its settling ends it.
#[cfg(unix)]
#[tokio::test]
async fn a_refused_steer_after_stop_keeps_the_cancelled_turn_until_it_settles() {
    let (_directory, mut actor) = started_fake_chat(
        r#"    *steer*) printf '{"type":"response","id":"%s","command":"prompt","success":false,"error":"Pi is busy"}\n' "$id" ;;
    *prompt-*) printf '{"type":"response","id":"%s","command":"prompt","success":true,"data":{"disposition":"started"}}\n' "$id" ;;
"#,
    )
    .await;
    send(&mut actor, "first").await;
    receive_next(&mut actor).await;
    assert!(actor.chats[&6].turn.is_running());
    let watched = actor.chats[&6].turn.cancellation();
    actor
        .chat_command(6, crate::ChatCommand::Abort)
        .await
        .unwrap();
    send(&mut actor, "also this").await;
    assert!(actor.chats[&6].turn.steered());
    receive_next(&mut actor).await;
    let chat = &actor.chats[&6];
    assert!(
        chat.turn.is_running(),
        "the refused steer leaves the turn running"
    );
    assert!(watched.load(std::sync::atomic::Ordering::Acquire));
    assert!(
        chat.turn
            .cancellation()
            .load(std::sync::atomic::Ordering::Acquire),
        "the steer did not reset Stop"
    );
    assert!(
        !chat_events(&actor)
            .iter()
            .any(|event| matches!(event, ChatEvent::TurnDone))
    );
    actor
        .receive(6, Incoming::Record(json!({"type":"agent_settled"})))
        .await;
    let chat = &actor.chats[&6];
    assert!(!chat.turn.is_running() && !chat.turn.steered());
    assert!(matches!(
        chat_events(&actor).last(),
        Some(ChatEvent::TurnDone)
    ));
    actor.shutdown().await;
}

#[cfg(unix)]
#[tokio::test]
async fn saved_tabs_restore_through_the_pi_actor_and_keep_the_active_session() {
    use crate::runtime::tabs::{SavedTabs, read_tabs, save_tabs};
    let directory = tempfile::tempdir().unwrap();
    let mut actor = test_actor(directory.path(), Catalog::new(Vec::new()).unwrap());
    super::super::fixture_tests::write_executable(
        &actor.files.executable,
        "#!/bin/sh\nwhile IFS= read -r line; do :; done\n",
    );
    std::fs::create_dir(&actor.files.sessions_dir).unwrap();
    let saved = SavedTabs {
        sessions: vec!["first-saved".into(), "second-saved".into()],
        active: Some("second-saved".into()),
    };
    save_tabs(&actor.files.tabs_file, &saved).unwrap();
    actor.restore(&Default::default()).await.unwrap();
    assert_eq!(actor.order, [1, 2]);
    assert_eq!(actor.active, 2);
    assert_eq!(actor.chats[&1].session_id.as_deref(), Some("first-saved"));
    assert_eq!(actor.chats[&2].session_id.as_deref(), Some("second-saved"));
    assert!(lock(&actor.events.items).iter().any(|event| matches!(event,
        Event::Restore { tabs, active: 2 } if tabs == &vec![(1, "first-saved".into()), (2, "second-saved".into())]
    )));
    let persisted = read_tabs(&actor.files.tabs_file);
    assert_eq!(persisted.sessions, saved.sessions);
    assert_eq!(persisted.active, saved.active);
    actor.shutdown().await;
}
