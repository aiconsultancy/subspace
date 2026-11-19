//! A fake agent exercising extension tolerance and bounded cancellation.
#![cfg(test)]
use super::cancellation::CANCEL_GRACE_PERIOD;
use super::fixture::send;
use super::fixture::{
    FAKE_AGENT, Fixture, actor, approve, chat, is_chat, ready, say, start, turn_done,
};
use super::{AcpActor, Incoming};
use crate::runtime::lock;
use crate::{ChatCommand, ChatEvent, ChatPreferences, Entry, ToolStatus};
use serde_json::json;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(10);

#[tokio::test]
async fn unsupported_requests_get_errors_and_the_prompt_still_ends() {
    for method in [
        "fs/read_text_file",
        "fs/write_text_file",
        "terminal/create",
        "future/request",
    ] {
        let script = FAKE_AGENT.replace("fs/read_text_file", method);
        let mut fixture = start(&script, None);
        fixture.wait_for(ready).await;
        say(&fixture, "Write my notes").await;
        approve(&mut fixture, true).await;
        fixture.wait_for(turn_done).await;
        assert!(
            fixture.seen.iter().any(|event| is_chat(event, |event| {
                matches!(event, ChatEvent::Delta { text, .. } if text == "No file access. ")
            })),
            "no method-not-found reply for {method}"
        );
    }
}

#[tokio::test]
async fn unknown_updates_and_warning_notifications_do_not_interrupt_a_turn() {
    let noise = r#"      update '{"sessionUpdate":"future_update","anything":{"nested":true}}'
      printf '%s\n' '{"jsonrpc":"2.0","method":"warning","params":{"message":"fixture marketplace warning"}}'
"#;
    let script = FAKE_AGENT.replace(
        "      text agent_message_chunk \"done.\"",
        &format!("{noise}      text agent_message_chunk \"done.\""),
    );
    let mut fixture = start(&script, None);
    fixture.wait_for(ready).await;
    for _ in 0..2 {
        say(&fixture, "Write my notes").await;
        approve(&mut fixture, true).await;
        fixture.wait_for(turn_done).await;
    }
    assert_eq!(
        fixture.seen.iter().filter(|event| turn_done(event)).count(),
        2
    );
    assert!(
        !fixture
            .seen
            .iter()
            .any(|event| is_chat(event, |event| matches!(event, ChatEvent::Error(_))))
    );
}

#[tokio::test]
async fn stop_replies_to_an_unanswered_permission_request_without_an_approval() {
    let script = FAKE_AGENT.replace(
        "      IFS= read -r answer",
        r#"      while IFS= read -r answer; do
        case "$answer" in *'"id":"perm-1"'*) break ;; esac
      done
      case "$answer" in *'"outcome":"cancelled"'*) ;; *) exit 7 ;; esac"#,
    );
    let mut fixture = start(&script, None);
    fixture.wait_for(ready).await;
    say(&fixture, "Write my notes").await;
    fixture
        .wait_for(|event| is_chat(event, |event| matches!(event, ChatEvent::Approval { .. })))
        .await;
    stop_cleanly(&mut fixture).await;
    assert_eq!(
        fixture.seen.iter().filter(|event| turn_done(event)).count(),
        1
    );
}

#[tokio::test]
async fn a_permission_request_after_stop_is_answered_as_cancelled() {
    let script = FAKE_AGENT.replace("reply \"$waiting\" '{\"stopReason\":\"cancelled\"}'", r#"
      printf '%s\n' '{"jsonrpc":"2.0","id":99,"method":"session/request_permission","params":{}}'
      IFS= read -r answer
      case "$answer" in *'"id":99'*'"outcome":"cancelled"'*) reply "$waiting" '{"stopReason":"cancelled"}' ;; *) exit 8 ;; esac"#);
    let mut fixture = start(&script, None);
    fixture.wait_for(ready).await;
    say(&fixture, "Wait for it").await;
    fixture
        .wait_for(|event| {
            is_chat(
                event,
                |event| matches!(event, ChatEvent::Entry(entry) if entry.entry.tool().is_some()),
            )
        })
        .await;
    stop_cleanly(&mut fixture).await;
    assert!(
        !fixture
            .seen
            .iter()
            .any(|event| is_chat(event, |event| matches!(
                event,
                ChatEvent::Approval { .. } | ChatEvent::Error(_)
            )))
    );
}

#[cfg(test)]
async fn stop_cleanly(fixture: &mut Fixture) {
    send(fixture, chat(ChatCommand::Abort)).await;
    fixture.wait_for(turn_done).await;
    assert!(
        !fixture
            .seen
            .iter()
            .any(|event| is_chat(event, |event| matches!(event, ChatEvent::Error(_))))
    );
}

#[tokio::test]
async fn every_prompt_response_ends_the_turn_including_errors_and_unknown_stop_reasons() {
    let normal =
        r#"reply "$id" '{"stopReason":"end_turn","usage":{"inputTokens":3,"outputTokens":5}}'"#;
    for reason in [
        "end_turn",
        "cancelled",
        "max_tokens",
        "max_turn_requests",
        "refusal",
        "future_reason",
        "rpc_error",
    ] {
        let reply = if reason == "rpc_error" {
            r#"printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32603,"message":"fixture error"}}\n' "$id""#.to_owned()
        } else {
            format!(r#"reply "$id" '{{"stopReason":"{reason}"}}'"#)
        };
        let mut fixture = start(&FAKE_AGENT.replace(normal, &reply), None);
        fixture.wait_for(ready).await;
        for _ in 0..2 {
            say(&fixture, "Write my notes").await;
            approve(&mut fixture, true).await;
            fixture.wait_for(turn_done).await;
        }
        assert_eq!(
            fixture.seen.iter().filter(|event| turn_done(event)).count(),
            2,
            "{reason}"
        );
    }
}

#[tokio::test]
async fn late_transcript_chunks_cannot_reopen_a_finished_turn() {
    let response =
        r#"reply "$id" '{"stopReason":"end_turn","usage":{"inputTokens":3,"outputTokens":5}}'"#;
    let trailing = r#"
      text agent_thought_chunk "Late thought "
      update '{"sessionUpdate":"session_info_update","title":"settled"}'"#;
    let mut fixture = start(
        &FAKE_AGENT.replace(response, &format!("{response}{trailing}")),
        None,
    );
    fixture.wait_for(ready).await;
    say(&fixture, "Write my notes").await;
    approve(&mut fixture, true).await;
    fixture.wait_for(turn_done).await;
    fixture
        .wait_for(|event| {
            is_chat(
                event,
                |event| matches!(event, ChatEvent::Title(title) if title == "settled"),
            )
        })
        .await;
    let end = fixture.seen.iter().position(turn_done).unwrap();
    assert!(
        !fixture.seen[end + 1..]
            .iter()
            .any(|event| is_chat(event, |event| matches!(event, ChatEvent::Delta { .. })))
    );
    send(&fixture, chat(ChatCommand::Abort)).await;
    fixture.wait_for(turn_done).await;
    assert_eq!(
        fixture.seen.iter().filter(|event| turn_done(event)).count(),
        2,
        "idle Stop clears any stale host turn"
    );
}

pub(super) async fn receive(actor: &mut AcpActor) -> Result<(), String> {
    let (id, incoming) = tokio::time::timeout(WAIT, actor.incoming.recv())
        .await
        .map_err(|_| "fixture output timed out")?
        .ok_or("fixture output closed")?;
    actor.receive(id, incoming).await;
    Ok(())
}

#[cfg(test)]
pub(super) async fn until_ready(actor: &mut AcpActor) -> Result<(), String> {
    while !actor.chats[&1].ready {
        receive(actor).await?;
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn waiting_agent(load: bool) -> String {
    FAKE_AGENT
        .replace(
            "reply \"$waiting\" '{\"stopReason\":\"cancelled\"}'",
            "printf '%s\\n' '{\"method\":\"cancel_seen\"}'",
        )
        .replace("\"loadSession\":true", &format!("\"loadSession\":{load}"))
        .replace(
            r#"    *'"method":"session/prompt"'*)"#,
            r#"    *'"method":"session/prompt"'*'Again'*)
      text agent_message_chunk "OK"
      update '{"sessionUpdate":"tool_call","toolCallId":"slow-1","title":"Next task","status":"completed"}'
      reply "$id" '{"stopReason":"end_turn"}' ;;
    *'"method":"session/prompt"'*)"#,
        )
}

#[cfg(test)]
pub(super) async fn start_waiting(actor: &mut AcpActor) -> Result<(), String> {
    actor.restore(&ChatPreferences::default()).await?;
    until_ready(actor).await?;
    let send = ChatCommand::Send {
        text: "Wait for it".into(),
        context: vec![],
        images: vec![],
    };
    actor.apply(chat(send), &Default::default()).await?;
    while !actor.chats[&1]
        .entries
        .iter()
        .any(|entry| entry.entry.tool().is_some())
    {
        receive(actor).await?;
    }
    actor
        .apply(chat(ChatCommand::Abort), &Default::default())
        .await?;
    loop {
        let (id, incoming) = tokio::time::timeout(WAIT, actor.incoming.recv())
            .await
            .map_err(|_| "fixture cancellation timed out")?
            .ok_or("fixture closed")?;
        let seen =
            matches!(&incoming, Incoming::Record(record) if record["method"] == "cancel_seen");
        actor.receive(id, incoming).await;
        if seen {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
pub(super) async fn expire_stop(actor: &mut AcpActor) {
    let deadline = actor.chats[&1].cancel_deadline;
    assert!(deadline.is_some());
    tokio::time::pause();
    let remaining = deadline.map_or(CANCEL_GRACE_PERIOD, |end| end - tokio::time::Instant::now());
    tokio::time::advance(remaining - Duration::from_nanos(1)).await;
    actor.expire_cancellations().await;
    assert!(actor.chats[&1].busy);
    tokio::time::advance(Duration::from_nanos(1)).await;
    actor.expire_cancellations().await;
    tokio::time::resume();
}

#[tokio::test]
async fn stop_expires_at_the_grace_deadline_and_a_replacement_accepts_the_next_message() {
    for load in [false, true] {
        let (_directory, mut actor, events) = actor(&waiting_agent(load), None, None);
        start_waiting(&mut actor).await.unwrap();
        let old_process = actor.chats[&1].process_id;
        expire_stop(&mut actor).await;
        assert!(!actor.chats[&1].busy);
        assert!(actor.chats[&1].recovering);
        assert_ne!(actor.chats[&1].process_id, old_process);
        assert_eq!(tool_states(&actor), [ToolStatus::Aborted]);
        let next = ChatCommand::Send {
            text: "Again".into(),
            context: vec![],
            images: vec![],
        };
        actor.apply(chat(next), &Default::default()).await.unwrap();
        actor
            .receive(old_process, Incoming::Exited("old process exited".into()))
            .await;
        actor
            .receive(
                old_process,
                Incoming::Record(json!({"id": 3, "result": {"stopReason": "end_turn"}})),
            )
            .await;
        until_ready(&mut actor).await.unwrap();
        assert!(actor.chats[&1].busy, "the queued next message was sent");
        assert!(
            actor.chats[&1]
                .entries
                .iter()
                .any(|entry| matches!(&entry.entry, Entry::User { text, .. } if text == "Again"))
        );
        while actor.chats[&1].busy {
            receive(&mut actor).await.unwrap();
        }
        assert_eq!(
            tool_states(&actor),
            [ToolStatus::Aborted, ToolStatus::Done],
            "reused tool ids preserve the stopped call"
        );
        assert_eq!(
            lock(&events.items)
                .iter()
                .filter(|event| turn_done(event))
                .count(),
            2
        );
        assert!(
            !lock(&events.items)
                .iter()
                .any(|event| is_chat(event, |event| matches!(event, ChatEvent::Error(_))))
        );
        actor.shutdown().await;
    }
}

#[cfg(test)]
fn tool_states(actor: &AcpActor) -> Vec<ToolStatus> {
    actor.chats[&1]
        .entries
        .iter()
        .filter_map(|entry| entry.entry.tool())
        .map(|call| call.status)
        .collect()
}
