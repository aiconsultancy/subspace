//! The fake ACP agent driving a real actor through setup, a streamed turn
//! with a tool call and a permission request, cancellation, model choice and
//! session replay.
use super::fixture::{
    FAKE_AGENT, approve, chat, is_chat, ready, rows, say, send, start, turn_done,
};
use crate::{ChatCommand, ChatEvent, Entry, Event, ToolStatus};

#[tokio::test]
async fn a_turn_streams_text_and_asks_before_the_agent_writes() {
    let mut fixture = start(FAKE_AGENT, None);
    let Event::Chat {
        event: ChatEvent::Ready { session_id },
        ..
    } = fixture.wait_for(ready).await
    else {
        unreachable!()
    };
    assert_eq!(session_id, "fake-1");
    fixture
        .wait_for(|event| matches!(event, Event::Models(models) if models.len() == 2))
        .await;
    fixture
        .wait_for(
            |event| matches!(event, Event::Commands(commands) if commands[0].name == "review"),
        )
        .await;

    say(&fixture, "Write my notes").await;
    let summary = approve(&mut fixture, true).await;
    assert_eq!(
        summary,
        "Fake agent asks to Write notes.txt (path=notes.txt)"
    );
    fixture.wait_for(turn_done).await;

    let rows = rows(&fixture.seen);
    assert!(matches!(&rows[0], Entry::User { text, .. } if text == "Write my notes"));
    assert_eq!(
        rows[1],
        Entry::Thinking {
            text: "No file access. ".into(),
            seconds: None
        },
        "an fs request the client did not advertise is refused"
    );
    assert_eq!(rows[2], Entry::Assistant("Writing ".into()));
    let call = rows[3].tool().unwrap();
    assert_eq!(
        (call.status, call.result.as_str()),
        (ToolStatus::Done, "wrote")
    );
    assert_eq!(rows[4], Entry::Assistant("done.".into()));
    assert!(fixture.seen.iter().any(|event| is_chat(event, |event| {
        matches!(event, ChatEvent::Usage(usage) if usage.output_tokens == Some(5))
    })));
    assert!(fixture.seen.iter().any(|event| is_chat(event, |event| {
        matches!(event, ChatEvent::Title(title) if title == "Write my notes")
    })));
}

#[tokio::test]
async fn a_denied_permission_fails_the_call() {
    let mut fixture = start(FAKE_AGENT, None);
    fixture.wait_for(ready).await;
    say(&fixture, "Write my notes").await;
    approve(&mut fixture, false).await;
    fixture.wait_for(turn_done).await;
    let rows = rows(&fixture.seen);
    let call = rows.iter().find_map(Entry::tool).unwrap();
    assert_eq!(call.status, ToolStatus::Failed);
}

#[tokio::test]
async fn stop_cancels_the_turn_and_marks_unfinished_calls() {
    let mut fixture = start(FAKE_AGENT, None);
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
    send(&fixture, chat(ChatCommand::Abort)).await;
    fixture.wait_for(turn_done).await;
    let rows = rows(&fixture.seen);
    assert_eq!(
        rows.iter().find_map(Entry::tool).unwrap().status,
        ToolStatus::Aborted
    );
    assert!(
        !fixture
            .seen
            .iter()
            .any(|event| is_chat(event, |event| matches!(event, ChatEvent::Error(_)))),
        "a cancelled turn is not an error: {:#?}",
        fixture.seen
    );
}

/// A message refused while no turn runs ends the turn the panel started for
/// it. ACP takes one message per turn, so one sent during a turn is refused
/// as a notice, and the turn goes on to its own single end.
#[tokio::test]
async fn a_refused_message_ends_a_turn_only_when_none_runs() {
    let mut fixture = start(FAKE_AGENT, None);
    fixture.wait_for(ready).await;
    send(
        &fixture,
        chat(ChatCommand::Send {
            text: "Look at this".into(),
            context: Vec::new(),
            images: vec![crate::ImageAttachment {
                id: 1,
                data: vec![1].into(),
                mime_type: "image/png".into(),
                display_name: "fixture.png".into(),
            }],
        }),
    )
    .await;
    let images_refused = |event: &Event| {
        is_chat(
            event,
            |event| matches!(event, ChatEvent::Error(error) if error.contains("images")),
        )
    };
    fixture.wait_for(images_refused).await;
    fixture.wait_for(turn_done).await;
    let refused = fixture.seen.iter().position(images_refused);
    assert!(
        refused.is_some_and(|index| fixture.seen.get(index + 1).is_some_and(turn_done)),
        "the end follows the refusal: {:#?}",
        fixture.seen
    );

    say(&fixture, "Wait for it").await;
    fixture
        .wait_for(|event| {
            is_chat(
                event,
                |event| matches!(event, ChatEvent::Entry(entry) if entry.entry.tool().is_some()),
            )
        })
        .await;
    say(&fixture, "Also this").await;
    fixture
        .wait_for(|event| is_chat(event, |event| matches!(event, ChatEvent::Error(error) if error.contains("still answering"))))
        .await;
    send(&fixture, chat(ChatCommand::Abort)).await;
    fixture.wait_for(turn_done).await;
    let ends = fixture.seen.iter().filter(|event| turn_done(event)).count();
    assert_eq!(ends, 2, "{:#?}", fixture.seen);
}

#[tokio::test]
async fn a_chosen_model_is_confirmed_by_the_agent() {
    let mut fixture = start(FAKE_AGENT, None);
    fixture.wait_for(ready).await;
    send(
        &fixture,
        chat(ChatCommand::SetModel {
            model: "deep".into(),
            effort: None,
        }),
    )
    .await;
    fixture
        .wait_for(|event| {
            is_chat(
                event,
                |event| matches!(event, ChatEvent::ModelConfirmed { model } if model == "deep"),
            )
        })
        .await;
    send(
        &fixture,
        chat(ChatCommand::SetModel {
            model: "missing".into(),
            effort: None,
        }),
    )
    .await;
    fixture
        .wait_for(|event| is_chat(event, |event| matches!(event, ChatEvent::Error(error) if error.contains("Choose a model"))))
        .await;
}

#[tokio::test]
async fn a_saved_tab_replays_its_session_as_history() {
    let mut fixture = start(
        FAKE_AGENT,
        Some(r#"{"sessions":["fake-1"],"active":"fake-1"}"#),
    );
    let history = fixture
        .wait_for(|event| {
            is_chat(
                event,
                |event| matches!(event, ChatEvent::History(rows) if !rows.is_empty()),
            )
        })
        .await;
    let Event::Chat {
        event: ChatEvent::History(rows),
        ..
    } = history
    else {
        unreachable!()
    };
    let rows: Vec<_> = rows.into_iter().map(|row| row.entry).collect();
    assert!(matches!(&rows[0], Entry::User { text, .. } if text == "Earlier question"));
    assert_eq!(rows[1], Entry::Assistant("Earlier answer".into()));
    fixture
        .wait_for(|event| {
            is_chat(
                event,
                |event| matches!(event, ChatEvent::Title(title) if title == "Earlier question"),
            )
        })
        .await;
    let Event::Chat {
        event: ChatEvent::Ready { session_id },
        ..
    } = fixture.wait_for(ready).await
    else {
        unreachable!()
    };
    assert_eq!(session_id, "fake-1");
}

#[tokio::test]
async fn an_agent_that_exits_during_setup_reports_its_diagnostics() {
    let mut fixture = start(
        "#!/bin/sh\nread -r line\necho 'not signed in' >&2\nexit 3\n",
        None,
    );
    let error = fixture
        .wait_for(|event| is_chat(event, |event| matches!(event, ChatEvent::Error(_))))
        .await;
    let Event::Chat {
        event: ChatEvent::Error(error),
        ..
    } = error
    else {
        unreachable!()
    };
    assert!(
        error.contains("Fake agent exited unexpectedly: not signed in"),
        "{error}"
    );
}
