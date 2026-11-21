use super::{ProjectViewEvent, mount_state};
use crate::project::{
    Component, ComponentKind, InputDeclaration, InputValue, LivePatch, LiveState,
    ProjectDefinition, ResultUpdate, ResultValue,
};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, TestAppContext, Window};

mod actions;

fn select_fixture() -> LiveState {
    let mut definition = ProjectDefinition::new("example", "python", "main.py");
    definition.inputs.insert(
        "choice".into(),
        InputDeclaration::String {
            default: "First".into(),
            options: vec!["First".into(), "Second".into()],
        },
    );
    definition.components = vec![Component::new(
        "choice",
        "Choice",
        ComponentKind::Select {
            input: "choice".into(),
        },
    )];
    LiveState::new(definition, "session-one").unwrap()
}

fn updated_options(state: &mut LiveState) {
    let mut patch = LivePatch::default();
    patch
        .options
        .insert("choice".into(), vec!["Second".into(), "Third".into()]);
    patch
        .values
        .insert("choice".into(), InputValue::String("Second".into()));
    state.apply("session-one", 0, patch).unwrap();
}

fn choose_next(window: &mut Window, cx: &mut App) {
    window.click("project-example-choice", cx);
    window.press("down", cx);
    window.press("enter", cx);
}

fn third_event() -> ProjectViewEvent {
    ProjectViewEvent::Input {
        key: "choice".into(),
        value: InputValue::String("Third".into()),
        session_id: "session-one".into(),
        expected_revision: 0,
    }
}

#[gpui_kit::test]
fn select_options_sync_is_silent_and_keyboard_selection_emits_current_value(
    cx: &mut TestAppContext,
) {
    let mut state = select_fixture();
    let (harness, events, cx) = mount_state(cx, state.clone());
    updated_options(&mut state);
    cx.update(|window, cx| {
        harness.update(cx, |harness, cx| {
            harness
                .view
                .update(cx, |view, cx| view.sync(&state, window, cx))
        })
    });
    cx.run_until_parked();
    assert!(events.borrow().is_empty(), "sync must not emit input");
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("project-example-choice").value(),
            Some("Second")
        );
        choose_next(window, cx);
    });
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), &[third_event()]);
}

#[gpui_kit::test]
fn select_confirmation_queued_before_sync_is_rejected_then_new_selection_works(
    cx: &mut TestAppContext,
) {
    let mut state = select_fixture();
    let (harness, events, cx) = mount_state(cx, state.clone());
    updated_options(&mut state);
    cx.update(|window, cx| {
        choose_next(window, cx);
        assert!(
            events.borrow().is_empty(),
            "confirmation must remain deferred until sync"
        );
        harness.update(cx, |harness, cx| {
            harness
                .view
                .update(cx, |view, cx| view.sync(&state, window, cx))
        });
    });
    cx.run_until_parked();
    assert!(
        events.borrow().is_empty(),
        "old confirmation must not cross sync"
    );
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("project-example-choice").value(),
            Some("Second"),
            "rejected confirmation must restore the accepted selection"
        );
        choose_next(window, cx);
    });
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), &[third_event()]);
}

#[gpui_kit::test]
fn result_only_sync_preserves_queued_select_confirmation(cx: &mut TestAppContext) {
    let mut definition = select_fixture().definition().clone();
    definition.components.push(Component::new(
        "summary",
        "Summary",
        ComponentKind::Text {
            text: "Waiting".into(),
        },
    ));
    let mut state = LiveState::new(definition, "session-one").unwrap();
    let (harness, events, cx) = mount_state(cx, state.clone());
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
    state.apply("session-one", 0, patch).unwrap();
    cx.update(|window, cx| {
        choose_next(window, cx);
        assert!(
            events.borrow().is_empty(),
            "confirmation must remain deferred until sync"
        );
        harness.update(cx, |harness, cx| {
            harness
                .view
                .update(cx, |view, cx| view.sync(&state, window, cx))
        });
    });
    cx.run_until_parked();
    assert_eq!(
        events.borrow().as_slice(),
        &[ProjectViewEvent::Input {
            key: "choice".into(),
            value: InputValue::String("Second".into()),
            session_id: "session-one".into(),
            expected_revision: 0,
        }]
    );
}
