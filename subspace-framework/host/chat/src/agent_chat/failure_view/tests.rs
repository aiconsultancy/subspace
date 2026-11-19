use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_agent::failure::TurnContext;
use subspace_agent::{
    ChatCommand, ChatEvent, Command, ContextRef, EntryKind, FailureCategory, ImageAttachment,
    TurnFailure,
};

use super::super::attention::apply_chat_event;
use super::super::tabs::{LastTurn, Tab};
use super::super::{InputIntent, Pending};
use super::fixture::parse;
use super::{ChatPanel, offers_reconnect, retry_allowed};
use crate::prefs;

fn failure(category: FailureCategory) -> TurnFailure {
    TurnFailure::stopped(
        category,
        "fixture failure",
        &TurnContext::default(),
        Some("1.0.105"),
    )
}

fn last_turn() -> LastTurn {
    LastTurn {
        text: "Fix my config".into(),
        context: vec![ContextRef {
            key: "fixture-key".into(),
            label: "Fixture item".into(),
            content: "fixture content".into(),
        }],
        images: vec![ImageAttachment {
            id: 7,
            data: vec![1, 2, 3].into(),
            mime_type: "image/png".into(),
            display_name: "fixture.png".into(),
        }],
    }
}

/// A tab whose last turn failed, ready for Retry.
fn failed_tab(category: FailureCategory) -> Tab {
    let mut tab = Tab::new(1);
    tab.ready();
    tab.last_turn = Some(last_turn());
    apply_chat_event(&mut tab, ChatEvent::Failure(Box::new(failure(category))));
    tab
}

/// The runtime reports a failed turn, then ends it.
#[test]
fn a_failed_turn_ends_silently_and_keeps_its_details() {
    let mut tab = Tab::new(1);
    tab.ready();
    tab.begin();
    tab.pending.push(Pending::Approval {
        id: "request".into(),
        summary: "fixture".into(),
    });
    apply_chat_event(
        &mut tab,
        ChatEvent::Failure(Box::new(failure(FailureCategory::NoResponse))),
    );
    let ended = apply_chat_event(&mut tab, ChatEvent::TurnDone);
    assert!(!tab.is_busy());
    assert_eq!(ended.attention, None, "a failed turn does not alert");
    assert!(tab.pending.is_empty());
    assert!(tab.is_ready(), "the runtime still has the chat");
    assert_eq!(
        tab.failure.as_ref().map(|shown| shown.failure.category),
        Some(FailureCategory::NoResponse)
    );
    tab.begin();
    assert!(tab.failure.is_none(), "a new turn clears the failure");
}

/// The watchdog ended the turn, then the runtime recovered: its output
/// resumes the turn, and the turn's own end clears busy again.
#[test]
fn a_recovered_runtime_resumes_the_turn_and_ends_it_again() {
    let mut tab = Tab::new(1);
    tab.ready();
    tab.begin();
    apply_chat_event(
        &mut tab,
        ChatEvent::Failure(Box::new(failure(FailureCategory::NotResponding))),
    );
    assert!(!tab.is_busy(), "Reconnect is not left disabled");
    assert!(!tab.is_ready());
    let delta = || ChatEvent::Delta {
        kind: EntryKind::Assistant,
        text: "late reply ".into(),
    };
    apply_chat_event(&mut tab, delta());
    assert!(tab.is_busy() && tab.running().is_some() && tab.failure.is_none());
    apply_chat_event(&mut tab, ChatEvent::TurnDone);
    assert!(!tab.is_busy() && tab.is_ready());
    let mut stopped = failed_tab(FailureCategory::Stopped);
    apply_chat_event(&mut stopped, delta());
    assert!(
        stopped.failure.is_some(),
        "a stopped runtime does not recover"
    );
}

#[test]
fn a_stopped_runtime_offers_reconnect_and_holds_retry_until_ready() {
    let mut tab = failed_tab(FailureCategory::Stopped);
    assert!(!tab.is_ready());
    assert!(offers_reconnect(&tab, false));
    assert!(!offers_reconnect(&tab, true), "the panel error offers it");
    assert!(!retry_allowed(&tab, false));
    tab.ready();
    assert!(retry_allowed(&tab, false));
    assert!(!retry_allowed(&tab, true), "not while the runtime is down");
    assert!(!offers_reconnect(&tab, false));
}

#[test]
fn retry_needs_a_message_and_an_idle_writable_chat() {
    let tab = failed_tab(FailureCategory::NoResponse);
    assert!(retry_allowed(&tab, false));
    assert!(!offers_reconnect(&tab, false));
    let changes: [fn(&mut Tab); 3] = [
        |tab| tab.resume(),
        |tab| tab.read_only = true,
        |tab| tab.last_turn = None,
    ];
    for change in changes {
        let mut tab = failed_tab(FailureCategory::NoResponse);
        change(&mut tab);
        assert!(!retry_allowed(&tab, false));
    }
}

/// The runtime stopped unexpectedly: no tab stays busy, so Reconnect is
/// enabled, and no tab is ready until a new runtime opens it.
#[gpui_kit::test]
fn an_unexpected_stop_ends_every_tabs_turn(cx: &mut TestAppContext) {
    let (chat, cx) = setup(cx);
    chat.update(cx, |chat, cx| {
        let mut second = Tab::new(2);
        second.ready();
        second.begin();
        chat.tab_mut().ready();
        chat.tab_mut().begin();
        chat.tabs.push(second);
        chat.poll(cx);
        assert!(chat.tabs.iter().all(Tab::is_busy), "a running runtime");
        chat.error = Some("The chat runtime stopped unexpectedly.".into());
        chat.poll(cx);
        for tab in &chat.tabs {
            assert!(!tab.is_busy() && !tab.is_ready());
            assert!(tab.running().is_none());
        }
    });
}

/// Send, steer, fail: Retry sends the message that started the turn.
#[gpui_kit::test]
fn retry_resends_the_message_that_started_the_failed_turn(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let (handle, mut commands) =
        subspace_agent::AgentHandle::test_offline(directory.path().into(), Vec::new());
    let (chat, cx) = setup(cx);
    chat.update(cx, |chat, cx| {
        chat.connected(Ok(handle));
        chat.tab_mut().ready();
        for text in ["Fix my config", "Also check the proxy"] {
            let mut submission =
                chat.prepare_submission(InputIntent::Message(text.into()), Vec::new());
            let command = submission.command.take().expect("a send");
            let sent = chat.send(command, cx);
            chat.finish_submission(submission, sent);
        }
        assert_eq!(
            chat.tab().last_turn.as_ref().map(|turn| turn.text.as_str()),
            Some("Fix my config"),
            "the steering message does not replace it"
        );
        apply_chat_event(
            chat.tab_mut(),
            ChatEvent::Failure(Box::new(failure(FailureCategory::NoResponse))),
        );
        apply_chat_event(chat.tab_mut(), ChatEvent::TurnDone);
        while commands.try_recv().is_ok() {}
        chat.retry(cx);
    });
    assert!(matches!(
        commands.try_recv(),
        Ok(Command::Chat {
            command: ChatCommand::Send { text, .. },
            ..
        }) if text == "Fix my config"
    ));
}

#[gpui_kit::test]
fn the_fixture_shows_a_ready_failure_without_a_runtime(cx: &mut TestAppContext) {
    let (chat, cx) = setup(cx);
    chat.update(cx, |chat, _| {
        chat.show_fixture(parse("no-response:details").expect("a fixture"));
    });
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(
            window.find("chat-failure-details-toggle").expanded(),
            Some(true)
        );
        assert_ne!(window.find("chat-failure-retry").disabled(), Some(true));
        assert!(window.try_find("chat-failure-reconnect").is_none());
        assert!(
            window.try_find("chat-reconnect").is_none(),
            "no startup error"
        );
        window.click("chat-failure-retry", cx);
    });
    settle(cx);
    chat.read_with(cx, |chat, _| {
        assert!(chat.handle.is_none() && chat.error.is_none());
        assert!(chat.tab().failure.is_some(), "Retry is a no-op");
        assert!(chat.tab().error.is_none());
        assert!(!chat.tab().is_busy());
    });
    chat.update(cx, |chat, _| {
        chat.show_fixture(parse("stopped").expect("a fixture"));
    });
    settle(cx);
    cx.update(|window, _| assert!(window.find("chat-failure-reconnect").visible()));
    chat.read_with(cx, |chat, _| {
        assert!(!retry_allowed(chat.tab(), chat.is_unavailable()));
    });
}

pub(super) fn setup(cx: &mut TestAppContext) -> (Entity<ChatPanel>, &mut VisualTestContext) {
    cx.update(|cx| {
        cx.set_reduce_motion(true);
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let (chat, cx) = cx.add_window_view(ChatPanel::new_copilot);
    cx.update(|window, cx| {
        window.replace_root(cx, |window, cx| {
            gpui_kit::base::Root::new(chat.clone(), window, cx)
        });
    });
    settle(cx);
    (chat, cx)
}

pub(super) fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn the_failure_panel_shows_its_parts_and_retry_resends_the_last_message(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let (handle, mut commands) =
        subspace_agent::AgentHandle::test_offline(directory.path().into(), Vec::new());
    let (chat, cx) = setup(cx);
    chat.update(cx, |chat, _| {
        chat.connected(Ok(handle));
        *chat.tab_mut() = failed_tab(FailureCategory::NoResponse);
    });
    settle(cx);
    cx.update(|window, cx| {
        for id in ["chat-failure", "chat-failure-title", "chat-failure-action"] {
            assert!(window.find(id).visible(), "{id}");
        }
        let toggle = window.find("chat-failure-details-toggle");
        assert_eq!(toggle.expanded(), Some(false), "Details start closed");
        assert_ne!(window.find("chat-failure-retry").disabled(), Some(true));
        assert!(window.find("chat-failure-copy").visible());
        assert!(window.try_find("chat-failure-reconnect").is_none());
        window.click("chat-failure-details-toggle", cx);
    });
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(
            window.find("chat-failure-details-toggle").expanded(),
            Some(true)
        );
        window.click("chat-failure-copy", cx);
    });
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert!(
        copied
            .as_deref()
            .is_some_and(|text| text.contains("Type: runtime") && text.contains("Runtime: 1.0.105")),
        "{copied:?}"
    );
    cx.update(|window, cx| window.click("chat-failure-retry", cx));
    settle(cx);
    let Ok(Command::Chat {
        chat: 1,
        command:
            ChatCommand::Send {
                text,
                context,
                images,
            },
    }) = commands.try_recv()
    else {
        panic!("Retry sends the last message");
    };
    let expected = last_turn();
    assert_eq!(text, expected.text);
    assert_eq!(context, expected.context);
    assert_eq!(images, expected.images);
    chat.read_with(cx, |chat, _| {
        assert!(chat.tab().failure.is_none(), "the new turn clears it");
        assert!(chat.tab().is_busy());
    });
    cx.update(|window, _| assert!(window.try_find("chat-failure").is_none()));
}
