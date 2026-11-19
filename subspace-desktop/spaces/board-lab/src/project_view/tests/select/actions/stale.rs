use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;

use super::{ProjectViewEvent, choose_next, fixture, mount_state, open, second_event, sync};
use crate::project::{InputValue, LivePatch, LiveState, ResultUpdate, ResultValue};

#[derive(Clone, Copy, Debug)]
enum Change {
    Session,
    Options,
    Revision,
    Value,
    Disabled,
}

fn changed_state(change: Change) -> LiveState {
    let mut state = fixture();
    match change {
        Change::Session => state.bind_session("session-two").unwrap(),
        Change::Revision => {
            state
                .set_value("amount", InputValue::Number(6.), 0)
                .unwrap();
        }
        _ => state.apply("session-one", 0, patch(change)).unwrap(),
    }
    state
}

fn patch(change: Change) -> LivePatch {
    let mut patch = LivePatch::default();
    match change {
        Change::Options => {
            patch
                .options
                .insert("choice".into(), vec!["First".into(), "Third".into()]);
        }
        Change::Value => {
            patch
                .values
                .insert("choice".into(), InputValue::String("Third".into()));
        }
        Change::Disabled => {
            patch.enabled.insert("choice".into(), false);
        }
        Change::Session | Change::Revision => {}
    }
    patch
}

#[gpui_kit::test]
fn old_sdk_confirmation_cannot_cross_its_binding_boundary(cx: &mut TestAppContext) {
    for change in [
        Change::Session,
        Change::Options,
        Change::Revision,
        Change::Value,
        Change::Disabled,
    ] {
        let (harness, events, cx) = mount_state(cx, fixture());
        let next = changed_state(change);
        cx.update(|window, cx| {
            open(&harness, window, cx);
            choose_next(window, cx);
            assert!(
                events.borrow().is_empty(),
                "SDK confirmation must be deferred"
            );
            sync(&harness, &next, window, cx);
        });
        cx.run_until_parked();
        assert!(
            events.borrow().is_empty(),
            "stale {change:?} confirmation escaped"
        );
        cx.update(|window, cx| {
            window.render_frame(cx);
            let expected = if matches!(change, Change::Value) {
                "Third"
            } else {
                "First"
            };
            assert_eq!(
                window.find("project-example-choice").value(),
                Some(expected),
                "{change:?}"
            );
            if matches!(change, Change::Disabled) {
                assert!(
                    window.focused(cx).is_none(),
                    "disabled select must not regain focus"
                );
            } else {
                assert_eq!(
                    window.find("project-example-choice").focused(),
                    Some(true),
                    "{change:?}"
                );
            }
        });
    }
}

#[gpui_kit::test]
fn fresh_sdk_action_after_session_sync_uses_the_new_binding(cx: &mut TestAppContext) {
    let (harness, events, cx) = mount_state(cx, fixture());
    cx.update(|window, cx| {
        open(&harness, window, cx);
        choose_next(window, cx);
        sync(&harness, &changed_state(Change::Session), window, cx);
    });
    cx.run_until_parked();
    assert!(events.borrow().is_empty());
    cx.update(|window, cx| {
        window.render_frame(cx);
        open(&harness, window, cx);
        choose_next(window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        events.borrow().as_slice(),
        &[ProjectViewEvent::Input {
            key: "choice".into(),
            value: InputValue::String("Second".into()),
            session_id: "session-two".into(),
            expected_revision: 0,
        }]
    );
}

#[gpui_kit::test]
fn repeated_binding_changes_keep_old_sdk_events_retired_and_restore_final_focus(
    cx: &mut TestAppContext,
) {
    let (harness, events, cx) = mount_state(cx, fixture());
    let session = changed_state(Change::Session);
    let mut next = session.clone();
    next.apply("session-two", 0, patch(Change::Options))
        .unwrap();
    cx.update(|window, cx| {
        open(&harness, window, cx);
        choose_next(window, cx);
        sync(&harness, &session, window, cx);
        sync(&harness, &next, window, cx);
    });
    cx.run_until_parked();
    assert!(events.borrow().is_empty());
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("project-example-choice").value(), Some("First"));
        assert_eq!(window.find("project-example-choice").focused(), Some(true));
    });
}

#[gpui_kit::test]
fn disabling_then_reenabling_does_not_revive_a_queued_sdk_confirmation(cx: &mut TestAppContext) {
    let state = fixture();
    let (harness, events, cx) = mount_state(cx, state.clone());
    cx.update(|window, cx| {
        open(&harness, window, cx);
        choose_next(window, cx);
        sync(&harness, &changed_state(Change::Disabled), window, cx);
        sync(&harness, &state, window, cx);
    });
    cx.run_until_parked();
    assert!(events.borrow().is_empty());
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("project-example-choice").value(), Some("First"));
        assert!(window.focused(cx).is_none());
        open(&harness, window, cx);
        choose_next(window, cx);
    });
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), &[second_event()]);
}

#[gpui_kit::test]
fn result_only_sync_keeps_the_sdk_emitter_and_queued_confirmation(cx: &mut TestAppContext) {
    let mut next = fixture();
    let (harness, events, cx) = mount_state(cx, next.clone());
    let mut patch = LivePatch::default();
    patch.results.insert(
        "summary".into(),
        ResultUpdate::new(
            "test",
            1,
            ResultValue::Text {
                text: "Updated".into(),
            },
        ),
    );
    next.apply("session-one", 0, patch).unwrap();
    cx.update(|window, cx| {
        open(&harness, window, cx);
        let old = harness.read(cx).view.read(cx).selects["choice"]
            .state
            .entity_id();
        choose_next(window, cx);
        sync(&harness, &next, window, cx);
        assert_eq!(
            harness.read(cx).view.read(cx).selects["choice"]
                .state
                .entity_id(),
            old
        );
    });
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), &[second_event()]);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("project-example-choice").value(),
            Some("Second")
        );
        assert_eq!(window.find("project-example-choice").focused(), Some(true));
    });
}
