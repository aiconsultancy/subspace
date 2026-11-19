//! A chat's turn bookkeeping against a fixture peer: a closed event stream,
//! a refused send, and the activity the watchdog reads.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use github_copilot_sdk::subscription::{RecvError, RecvErrorKind};
use github_copilot_sdk::types::SessionEvent;
use tempfile::TempDir;

use super::super::chat::{Chat, Env};
use super::super::lock;
use super::super::replay_fixture::ReplayPeer;
use super::super::watchdog::TIMING;
use super::super::watchdog::fence::{expire, stalled};
use super::Actor;
use super::tests::replay_actor;
use crate::{ChatEvent, ChatId, Event, FailureCategory};

/// An actor with chat 4 open on a fixture peer, its startup events drained.
/// The directory and the peer must outlive the actor's client.
struct Opened {
    _directory: TempDir,
    peer: ReplayPeer,
    actor: Actor,
}

async fn opened() -> Opened {
    let directory = tempfile::tempdir().unwrap();
    let peer = ReplayPeer::new(directory.path());
    let mut actor = replay_actor(&peer, directory.path());
    tokio::time::timeout(
        Duration::from_secs(10),
        actor.open_replay(4, "fixture-task"),
    )
    .await
    .unwrap()
    .unwrap();
    chat_events(&actor, 4);
    Opened {
        _directory: directory,
        peer,
        actor,
    }
}

/// What every chat is given, beside chat 4 itself.
fn chat_four(actor: &mut Actor) -> (Env<'_>, &mut Chat) {
    let Actor {
        client,
        setup,
        events,
        chats,
        ..
    } = actor;
    let env = Env {
        client,
        setup,
        events,
    };
    (env, chats.get_mut(&4).unwrap())
}

/// The chat events the actor queued for `chat`, in order.
fn chat_events(actor: &Actor, chat: ChatId) -> Vec<ChatEvent> {
    lock(&actor.events.items)
        .drain(..)
        .filter_map(|event| match event {
            Event::Chat { chat: id, event } if id == chat => Some(event),
            _ => None,
        })
        .collect()
}

/// When the runtime process exits, each chat's event stream closes. A busy
/// chat ends its turn with a Stopped failure instead of staying busy.
#[tokio::test]
async fn a_closed_event_stream_ends_a_busy_chat_with_a_stopped_failure() {
    let mut fixture = opened().await;
    let actor = &mut fixture.actor;
    actor.chats.get_mut(&4).unwrap().busy = true;
    actor
        .receive(4, Err(RecvError::from(RecvErrorKind::Closed)))
        .await;
    let events = chat_events(actor, 4);
    assert!(
        matches!(
            events.as_slice(),
            [ChatEvent::Failure(failure), ChatEvent::TurnDone]
                if failure.category == FailureCategory::Stopped
                    && failure.error_type == "runtime_stopped"
        ),
        "{events:?}"
    );
    assert!(!actor.chats[&4].busy, "the chat accepts a new message");
    assert!(
        !lock(&actor.setup.activity)[&4].busy,
        "the watchdog sees the turn ended"
    );
    actor
        .receive(4, Err(RecvError::from(RecvErrorKind::Closed)))
        .await;
    let idle = chat_events(actor, 4);
    assert!(
        matches!(idle.as_slice(), [ChatEvent::Failure(_)]),
        "an idle chat reports the stop without ending a turn: {idle:?}"
    );
}

/// A `session.error` is reported and ends the turn before the policy check
/// that follows it, so a runtime that hangs on that check holds back
/// nothing the chat shows.
#[tokio::test]
async fn a_session_error_is_reported_before_the_policy_check() {
    let mut fixture = opened().await;
    fixture.actor.chats.get_mut(&4).unwrap().busy = true;
    fixture.peer.hold.store(true, Ordering::SeqCst);
    let event: SessionEvent = serde_json::from_value(serde_json::json!({
        "id": "fixture-error",
        "timestamp": "2026-11-06T00:00:00Z",
        "parentId": null,
        "type": "session.error",
        "data": {"errorType": "query", "message": "No response was returned."}
    }))
    .unwrap();
    let checked = tokio::time::timeout(
        Duration::from_millis(200),
        fixture.actor.receive(4, Ok(event)),
    )
    .await;
    assert!(checked.is_err(), "the policy check waits on the held peer");
    let events = chat_events(&fixture.actor, 4);
    assert!(
        matches!(
            events.as_slice(),
            [ChatEvent::Failure(failure), ChatEvent::TurnDone]
                if failure.category == FailureCategory::NoResponse
        ),
        "{events:?}"
    );
    fixture.peer.hold.store(false, Ordering::SeqCst);
}

/// The fixture peer answers no policy call, so the access check fails. The
/// chat reports a structured failure and ends the turn, not a bare string.
#[tokio::test]
async fn a_failed_access_check_is_a_failure_that_ends_the_turn() {
    let mut fixture = opened().await;
    let (env, chat) = chat_four(&mut fixture.actor);
    let result = chat
        .send_message(&env, "Fix my config".into(), Vec::new(), Vec::new())
        .await;
    assert_eq!(result, Ok(()), "no second, string error");
    assert!(!chat.busy);
    let events = chat_events(&fixture.actor, 4);
    assert!(
        matches!(
            events.as_slice(),
            [ChatEvent::Failure(failure), ChatEvent::TurnDone]
                if failure.category == FailureCategory::Unknown
                    && failure.error_type == "access_check"
                    && failure.message.contains("Enterprise policy could not be verified")
        ),
        "{events:?}"
    );
}

/// The connection closes before the access check: that is a stopped
/// runtime, not a refused check.
#[tokio::test]
async fn a_closed_connection_during_the_access_check_is_a_stopped_runtime() {
    let mut fixture = opened().await;
    fixture.actor.client.force_stop();
    let (env, chat) = chat_four(&mut fixture.actor);
    let result = chat
        .send_message(&env, "Fix my config".into(), Vec::new(), Vec::new())
        .await;
    assert_eq!(result, Ok(()));
    let events = chat_events(&fixture.actor, 4);
    assert!(
        matches!(
            events.as_slice(),
            [ChatEvent::Failure(failure), ChatEvent::TurnDone]
                if failure.category == FailureCategory::Stopped
        ),
        "{events:?}"
    );
}

/// A new turn gets the whole wait, and a turn the watchdog ended does not
/// make the next message steer it.
#[tokio::test]
async fn a_new_turn_restarts_the_wait_and_takes_over_a_watchdog_stop() {
    let mut fixture = opened().await;
    let (env, chat) = chat_four(&mut fixture.actor);
    chat.last_event = Instant::now() - Duration::from_secs(600);
    chat.begin_turn(&env, false, 2, Vec::new());
    let shared = lock(&env.setup.activity)[&4].clone();
    assert!(shared.busy);
    assert_eq!(shared.turn.attachments, 2);
    assert!(!stalled(Instant::now(), &shared, TIMING.stall_after));
    let observed = [(4, chat.fence.current)];
    let later = Instant::now() + TIMING.stall_after;
    assert_eq!(
        expire(
            &mut lock(&env.setup.activity),
            &observed,
            later,
            TIMING.stall_after
        )
        .len(),
        1
    );
    chat.reconcile(&env);
    assert!(!chat.busy, "the next message starts a new turn");
    assert_eq!(chat.turn.sent, None);
    assert!(!chat.admit(&env, "session.idle"), "the watchdog ended it");
    let ended = chat.fence.current;
    assert!(chat.admit(&env, "assistant.message_delta"), "late output");
    assert!(chat.busy, "the recovered turn runs again");
    assert_eq!(chat.fence.current, ended + 1);
    assert!(
        lock(&env.setup.activity)[&4].busy,
        "and the watchdog sees it"
    );
    assert!(chat.admit(&env, "session.idle"), "its own end is reported");
}
