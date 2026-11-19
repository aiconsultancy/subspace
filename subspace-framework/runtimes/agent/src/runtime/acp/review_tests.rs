//! Accepted cross-model findings, exercised through the existing fake agent.
#![cfg(test)]
use super::fixture::{actor, chat, is_chat, turn_done};
use super::resilience_tests::{expire_stop, receive, start_waiting, until_ready, waiting_agent};
use super::{AcpActor, ChatCommand, ChatEvent, Incoming, json};
use crate::runtime::lock;
use crate::runtime::pi::fixture_tests::write_executable;
use std::time::Duration;

#[cfg(test)]
async fn again(actor: &mut AcpActor) -> Result<(), String> {
    actor
        .apply(
            chat(ChatCommand::Send {
                text: "Again".into(),
                context: vec![],
                images: vec![],
            }),
            &Default::default(),
        )
        .await?;
    until_ready(actor).await?;
    while actor.chats[&1].busy {
        receive(actor).await?;
    }
    Ok(())
}

#[cfg(test)]
async fn failed_restart(handshake: bool) -> Result<(), String> {
    let (directory, mut actor, events) = actor(&waiting_agent(true), None, None);
    start_waiting(&mut actor).await?;
    let title = actor.chats[&1].title.clone();
    let created = actor.chats[&1].created_at.clone();
    let executable = actor.launch.executable.clone();
    actor.launch.executable = directory.path().join("replacement");
    if handshake {
        write_executable(
            &actor.launch.executable,
            "#!/bin/sh\nread -r line\nprintf '%s\\n' '{\"id\":1,\"error\":{\"code\":-32603,\"message\":\"fixture setup failed\"}}'\n",
        );
    }
    expire_stop(&mut actor).await;
    while actor.chats.get(&1).is_some_and(|chat| !chat.ready) {
        receive(&mut actor).await?;
    }
    assert!(
        actor.order.iter().all(|id| actor.chats.contains_key(id)),
        "order must contain only live chats"
    );
    assert!(lock(&events.items).iter().any(|event| is_chat(
        event,
        |event| matches!(event, ChatEvent::Error(error) if error.contains("restart"))
    )));
    actor.launch.executable = executable;
    again(&mut actor).await?;
    assert_eq!(actor.chats[&1].title, title);
    assert_eq!(actor.chats[&1].created_at, created);
    assert!(actor.chats[&1].entries.iter().any(
        |entry| matches!(&entry.entry, crate::Entry::User { text, .. } if text == "Wait for it")
    ));
    actor.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn failed_spawn_retains_the_conversation_and_next_message_retries() {
    let result = failed_restart(false).await;
    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn failed_handshake_retains_the_conversation_and_next_message_retries() {
    let result = failed_restart(true).await;
    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn process_exit_during_stop_recovers_once_immediately_or_before_the_deadline() {
    for near_deadline in [false, true] {
        let (_directory, mut actor, events) = actor(&waiting_agent(true), None, None);
        start_waiting(&mut actor).await.unwrap();
        let old = actor.chats[&1].process_id;
        if near_deadline {
            tokio::time::pause();
            let deadline = actor.chats[&1].cancel_deadline.unwrap();
            tokio::time::advance(deadline - tokio::time::Instant::now() - Duration::from_nanos(1))
                .await;
        }
        actor.chats.get_mut(&1).unwrap().child.start_kill().unwrap();
        if near_deadline {
            tokio::time::resume();
        }
        while actor
            .chats
            .get(&1)
            .is_some_and(|chat| chat.process_id == old)
        {
            receive(&mut actor).await.unwrap();
        }
        assert!(
            actor.chats.contains_key(&1),
            "exit during Stop must start a replacement"
        );
        until_ready(&mut actor).await.unwrap();
        assert_eq!(
            lock(&events.items)
                .iter()
                .filter(|event| turn_done(event))
                .count(),
            1
        );
        again(&mut actor).await.unwrap();
        assert_eq!(
            lock(&events.items)
                .iter()
                .filter(|event| turn_done(event))
                .count(),
            2
        );
        actor.shutdown().await;
    }
}

#[tokio::test]
async fn an_old_permission_callback_cannot_answer_a_reused_wire_id() {
    let (_directory, mut actor, _events) = actor(&waiting_agent(true), None, None);
    start_waiting(&mut actor).await.unwrap();
    let events = actor.events.clone();
    let chat = actor.chats.get_mut(&1).unwrap();
    chat.cancelled = false;
    let params = json!({"options":[{"optionId":"yes","kind":"allow_once"},{"optionId":"no","kind":"reject_once"}]});
    let old = chat
        .agent_request(&json!("reused"), "session/request_permission", &params)
        .await
        .unwrap()
        .unwrap();
    chat.cancel_deadline = None;
    chat.cancel().await.unwrap();
    chat.turn_ended(Ok(&json!({"stopReason":"cancelled"})), &events)
        .await
        .unwrap();
    chat.send("Wait again", &[], &[], &events).await.unwrap();
    let new = chat
        .agent_request(&json!("reused"), "session/request_permission", &params)
        .await
        .unwrap()
        .unwrap();
    let process_id = chat.process_id;
    actor
        .receive(
            process_id,
            Incoming::Permission {
                ask: old,
                allowed: true,
            },
        )
        .await;
    assert_eq!(
        actor.chats[&1].permissions.len(),
        1,
        "the new request must still await its own decision"
    );
    actor
        .receive(
            process_id,
            Incoming::Permission {
                ask: new,
                allowed: false,
            },
        )
        .await;
    assert!(actor.chats[&1].permissions.is_empty());
    actor.shutdown().await;
}
